# SPDX-License-Identifier: Apache-2.0
"""Run a list of change-set replays on the live lifetime, each under its own stages.

Each step names a label, the Focus and Context profiles (``fixture.py stage``
names, or none) and the baseline result whose recorded first Answer calls it
replays. For every step the script sets the stages (``set_stages.py``,
verified live), then runs ``cs_replay.py`` with the change set's Answer
budget. A one-line summary per step goes to ``<out>/replays-summary.jsonl``.
A failed step is recorded and the remaining steps continue, because the
steps are independent measurements.

``python -I -B run_cs_replays.py --series baseline --repos airk --out <R>/change-set-1/replays/airk --plan default``
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

SW = "focus-static1024-cpu-cs1-software-w16"
DEFAULT_PLAN = [  # label, focus, context, baseline result
    ("all-noModel", None, None, "noModel"),
    ("all-focus-w16", SW, None, "focus-cpu"),
    ("all-focus-cand1", "focus-static1024-cpu-cand1-software-w16", None, "focus-cpu"),
    ("all-compact", SW, "context-compact-cpu-cs1-provisional-read", "compact-cpu"),
    ("all-large", SW, "context-large-cpu-cs1-provisional-read", "large-cpu"),
    ("loo-focus-publisher", "focus-static1024-cpu-cs1-publisher-w16", None, "focus-cpu"),
    ("loo-compact-original-context", SW, "context-compact-cpu", "compact-cpu"),
]
CS1_BUDGET = {"budget": {"calls": 12, "rounds": 3, "fanout": 12, "candidates": 32, "max_bytes": 524288,
                         "input_tokens": 8192, "deadline_ms": 120000}}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--series", required=True)
    parser.add_argument("--repos", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--plan", default="default", help="'default' or a JSON list of [label, focus, context, result]")
    parser.add_argument("--only", help="comma-separated labels to run")
    parser.add_argument("--answer-defaults", default=json.dumps(CS1_BUDGET))
    args = parser.parse_args(argv)
    plan = DEFAULT_PLAN if args.plan == "default" else [tuple(step) for step in json.loads(args.plan)]
    if args.only:
        plan = [step for step in plan if step[0] in set(args.only.split(","))]
    args.out.mkdir(parents=True, exist_ok=True)
    py = [sys.executable, "-I", "-B"]
    status = 0
    for label, focus, context, result in plan:
        began = time.monotonic()
        stages = {"focus": focus, "context": context}
        steps = [("stages", [*py, str(HERE / "set_stages.py"), "--focus", focus or "none", "--context",
                             context or "none", "--log-dir", str(args.out)]),
                 ("replay", [*py, str(HERE / "cs_replay.py"), "--series", args.series, "--configurations", result,
                             "--label", label, "--out", str(args.out), "--repos", args.repos, "--stages",
                             json.dumps(stages), "--answer-defaults", args.answer_defaults])]
        outcome = {"label": label, "stages": stages, "result": result}
        for name, command in steps:
            proc = subprocess.run(command, capture_output=True, stdin=subprocess.DEVNULL, cwd=str(common.REPO_ROOT),
                                  creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            common.write_bytes(args.out / "steps" / f"{label}-{name}.log", proc.stdout + b"\n--- stderr ---\n" + proc.stderr)
            if proc.returncode != 0:
                outcome[name + "_failed"] = proc.returncode
                status = 1
                break
        report = args.out / label / "cs-replay.json"
        if report.is_file() and "replay_failed" not in outcome and "stages_failed" not in outcome:
            data = common.read_json(report)
            outcome.update({k: data.get(k) for k in ("calls", "ok", "essential_covered", "recorded_essential_covered",
                                                     "essential_total", "same_evidence", "elapsed_ms_mean",
                                                     "recorded_elapsed_ms_mean")},
                           tokens_unchanged=all(v.get("unchanged") for v in (data.get("tokens") or {}).values()))
        outcome["elapsed_s"] = round(time.monotonic() - began, 1)
        common.append_jsonl(args.out / "replays-summary.jsonl", {"at": common.now_iso(), **outcome})
        print(json.dumps(outcome), flush=True)
    return status


if __name__ == "__main__":
    sys.exit(main())
