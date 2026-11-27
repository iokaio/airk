# SPDX-License-Identifier: Apache-2.0
"""Run the agent cost sessions of one series and report medians.

Evaluation tooling for the Gather Context strategy of 27 November 2026. It
runs ``agent_cost_run.py`` once for every task, arm and repeat of a series,
a few sessions at a time, and skips a session whose folder already exists,
so an interrupted series resumes. The working set is the 6 broad tasks and
the 14 narrow tasks; ``--tasks`` narrows it.

``report`` reads the finished sessions and prints, per task and arm, the
median cost and round trips over the repeats, and, where both arms exist,
the percentage lower cost with the tool. The combined figure counts a narrow
task four times a broad one. It reports and does not judge: correctness is
recorded separately against each task's frozen truth.

    python -B agent_cost_batch.py run --campaign <dir> --series <name> --model claude-sonnet-5-5 \
        --arms plain [--handle <handle.json> --instructions-file <text>] [--repeats 3] [--workers 3]
    python -B agent_cost_batch.py report --campaign <dir> --series <name>
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import statistics
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
BROAD = {"bookbuilder": ("C3", "A1", "T1"), "munarium": ("C2", "A1", "T1")}
NARROW = ("N-C1", "N-C2", "N-C3", "N-C4", "N-A1", "N-A2", "N-W1")
SNAPSHOT = {"bookbuilder": ("bookbuilder-v2-238b22a722ce", "bookbuilder-v2"), "munarium": ("munarium-9948345d1d14", "munarium")}
FIXTURE = "fixture-win-minilm-cuda-l7-cs1-bundle-cuda-r2"
NARROW_WEIGHT = 4


def working_set(campaign: pathlib.Path) -> list[dict]:
    rows = []
    for repository in sorted(BROAD):
        folder, index = SNAPSHOT[repository]
        snapshot = campaign / FIXTURE / "sources" / folder
        for slot in BROAD[repository]:
            rows.append({"id": f"{repository}-{slot}", "set": "broad", "repository": repository, "index": index,
                         "task": campaign / "scenarios" / repository / f"{slot}.json", "snapshot": snapshot})
        for slot in NARROW:
            rows.append({"id": f"{repository}-{slot}", "set": "narrow", "repository": repository, "index": index,
                         "task": campaign / "scenarios-narrow" / repository / f"{slot}.json", "snapshot": snapshot})
    return rows


def run(args) -> int:
    tasks = [row for row in working_set(args.campaign) if not args.tasks or row["id"] in args.tasks]
    series = args.campaign / "agent-cost" / args.series
    series.mkdir(parents=True, exist_ok=True)
    jobs = []
    for repeat in range(1, args.repeats + 1):
        for row in tasks:
            for arm in args.arms:
                out = series / f"{row['id']}.{arm}.r{repeat}"
                if out.exists():
                    continue
                command = [sys.executable, "-B", str(HERE / "agent_cost_run.py"), "--task", str(row["task"]),
                           "--snapshot", str(row["snapshot"]), "--arm", arm, "--model", args.model, "--output", str(out)]
                if arm == "tool":
                    command += ["--handle", str(args.handle), "--repo", row["index"]]
                    if args.instructions_file:
                        command += ["--instructions-file", str(args.instructions_file)]
                    if args.dose > 0:
                        command += ["--judge", str(args.judge), "--dose", str(args.dose)]
                jobs.append((out.name, command))
    print(f"{len(jobs)} session(s) to run", flush=True)

    def one(job):
        name, command = job
        done = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace")
        lines = (done.stdout or "").strip().splitlines()
        return f"{name}: {lines[-1] if lines else 'no output'}" + ("" if done.returncode == 0 else f" [exit {done.returncode}] {(done.stderr or '')[-300:]}")

    with concurrent.futures.ThreadPoolExecutor(args.workers) as pool:
        for line in pool.map(one, jobs):
            print(line, flush=True)
    return 0


def sessions(series: pathlib.Path) -> dict[tuple[str, str], list[dict]]:
    found: dict[tuple[str, str], list[dict]] = {}
    for path in sorted(series.glob("*/session.json")):
        task_id, arm, _ = path.parent.name.rsplit(".", 2)
        found.setdefault((task_id, arm), []).append(json.loads(path.read_text(encoding="utf-8")))
    return found


def medians(rows: list[dict]) -> dict:
    done = [row for row in rows if row.get("completed") and row.get("cost_usd") is not None]
    if not done:
        return {"runs": len(rows), "completed": 0}
    tokens = lambda key: statistics.median(row["tokens"].get(key) or 0 for row in done)  # noqa: E731
    return {"runs": len(rows), "completed": len(done), "cost_usd": round(statistics.median(row["cost_usd"] for row in done), 4),
            "round_trips": statistics.median(row["round_trips"] for row in done),
            "elapsed_s": round(statistics.median((row.get("elapsed_ms") or 0) / 1000 for row in done), 1),
            "cache_read": tokens("cache_read"), "cache_write": tokens("cache_write"), "output": tokens("output"),
            "gather_calls": statistics.median(row["gather_context"]["calls"] for row in done),
            "gather_bytes": statistics.median(row["gather_context"]["reply_bytes"] for row in done)}


def report(args) -> int:
    series = args.campaign / "agent-cost" / args.series
    found = sessions(series)
    weights = {row["id"]: (NARROW_WEIGHT if row["set"] == "narrow" else 1) for row in working_set(args.campaign)}
    table, saved = [], []
    for task_id in sorted({key[0] for key in found}):
        row = {"task": task_id, "weight": weights.get(task_id, 1)}
        for arm in ("plain", "tool"):
            if (task_id, arm) in found:
                row[arm] = medians(found[(task_id, arm)])
        plain, tool = row.get("plain", {}), row.get("tool", {})
        if plain.get("cost_usd") and tool.get("cost_usd") is not None:
            row["cost_lower_pct"] = round(100 * (1 - tool["cost_usd"] / plain["cost_usd"]), 1)
            row["round_trips_lower_pct"] = round(100 * (1 - tool["round_trips"] / plain["round_trips"]), 1)
            saved.append((row["weight"], row["cost_lower_pct"], row["round_trips_lower_pct"]))
        table.append(row)
        text = f"{task_id:<22} w={row['weight']}"
        for arm in ("plain", "tool"):
            if arm in row and row[arm].get("completed"):
                m = row[arm]
                text += (f" | {arm}: ${m['cost_usd']:.4f} trips={m['round_trips']} {m['elapsed_s']}s "
                         f"({m['completed']}/{m['runs']})" + (f" calls={m['gather_calls']} bytes={m['gather_bytes']}" if arm == "tool" else ""))
            elif arm in row:
                text += f" | {arm}: 0/{row[arm]['runs']} completed"
        if "cost_lower_pct" in row:
            text += f" | cost lower {row['cost_lower_pct']}% trips lower {row['round_trips_lower_pct']}%"
        print(text)
    summary = {}
    if saved:
        total = sum(weight for weight, _, _ in saved)
        summary = {"tasks_compared": len(saved),
                   "weighted_cost_lower_pct": round(sum(weight * cost for weight, cost, _ in saved) / total, 1),
                   "weighted_round_trips_lower_pct": round(sum(weight * trips for weight, _, trips in saved) / total, 1)}
        print(json.dumps(summary))
    (series / "report.json").write_text(json.dumps({"schema": "airk.private.agent-cost-report/1", "series": args.series,
                                                     "tasks": table, "summary": summary}, indent=2, sort_keys=True) + "\n",
                                        encoding="utf-8", newline="\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("run", "report"))
    parser.add_argument("--campaign", required=True, type=pathlib.Path)
    parser.add_argument("--series", required=True)
    parser.add_argument("--model")
    parser.add_argument("--arms", type=lambda value: value.split(","), default=["plain"])
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--workers", type=int, default=3)
    parser.add_argument("--handle", type=pathlib.Path)
    parser.add_argument("--instructions-file", type=pathlib.Path)
    parser.add_argument("--tasks", nargs="*")
    parser.add_argument("--judge", type=pathlib.Path, help="state file of a running arbiter_judge_service.py")
    parser.add_argument("--dose", type=int, default=0, help="judgments a call the model spends choosing passages")
    args = parser.parse_args(argv)
    if args.command == "report":
        return report(args)
    if not args.model or ("tool" in args.arms and args.handle is None):
        parser.error("run needs --model, and --handle for the tool arm")
    if args.dose > 0 and args.judge is None:
        parser.error("--dose needs --judge")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
