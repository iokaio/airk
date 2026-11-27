# SPDX-License-Identifier: Apache-2.0
"""Run the agent cost sessions of one series and report medians.

Evaluation tooling for the Gather Context strategy of 27 November 2026. It
runs ``agent_cost_run.py`` once for every task, arm and repeat of a series,
a few sessions at a time, and skips a session whose folder already exists,
so an interrupted series resumes. The working set is the 6 broad tasks and
the 14 narrow tasks; ``--tasks`` narrows it.

``report`` reads the finished sessions and prints, per task and arm, the
median cost and round trips over the repeats, and, where both arms exist,
the percentage lower cost of each other arm (``tool``, ``oracle``) against
the plain arm, at one price for every cache write (see ``one_price``): the
agent's own reported cost depends on which cache lifetime its account was
on when the session ran, which is not a property of the arm. The combined figure counts a narrow
task four times a broad one. It reports and does not judge: correctness is
recorded separately against each task's frozen truth.

    python -B agent_cost_batch.py run --campaign <dir> --series <name> --model claude-sonnet-5-5 \
        --arms plain [--handle <handle.json> --instructions-file <text>] [--repeats 3] [--workers 3]
    python -B agent_cost_batch.py report --campaign <dir> --series <name> [--baseline <series with the plain arm>]
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
                if args.max_round_trips:
                    command += ["--max-round-trips", str(args.max_round_trips)]
                if arm in ("tool", "oracle", "live"):
                    # Every arm that offers the tool gets the same description and guidance options. Until
                    # 27 November 2026 these were passed to the live arm only, so oracle sessions ran with no
                    # server guidance while live sessions ran with the build's.
                    command += ["--repo", row["index"]]
                    if args.handle:
                        command += ["--handle", str(args.handle)]
                    if args.tool_file:
                        command += ["--tool-file", str(args.tool_file)]
                    if args.instructions_file:
                        command += ["--instructions-file", str(args.instructions_file)]
                    if args.description_file:
                        command += ["--description-file", str(args.description_file)]
                    if args.dose > 0:
                        command += ["--judge", str(args.judge), "--dose", str(args.dose)]
                if arm == "oracle":
                    command += ["--oracle-layout", args.oracle_layout] + (["--oracle-trust"] if args.oracle_trust else [])
                if arm == "live":
                    command += ["--live-map", str(args.maps / f"{row['snapshot'].name}.map.json"), "--live-layout", args.live_layout]
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


def final_usage(folder: pathlib.Path) -> dict | None:
    """The session's last result event: its reported cost and the five token counts the cost is made of."""
    stream = folder / "stream.jsonl"
    if not stream.is_file():
        return None
    result = None
    for line in stream.read_text(encoding="utf-8", errors="replace").splitlines():
        if '"result"' in line:
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if event.get("type") == "result" and isinstance(event.get("usage"), dict):
                result = event
    if result is None or result.get("total_cost_usd") is None:
        return None
    usage, written = result["usage"], result["usage"].get("cache_creation") or {}
    return {"cost": result["total_cost_usd"], "input": usage.get("input_tokens") or 0, "output": usage.get("output_tokens") or 0,
            "cache_read": usage.get("cache_read_input_tokens") or 0,
            "write_5m": written.get("ephemeral_5m_input_tokens") or 0, "write_1h": written.get("ephemeral_1h_input_tokens") or 0}


def one_price(rows: list[dict]) -> dict:
    """Give every session a cost at one price for all cache writes, and say what was fitted.

    A session's reported cost prices a cache write by its lifetime: an hour costs more than five minutes. The
    lifetime is set by the account's state when the session ran, so two arms run at different times can differ in
    cost for that reason alone (found on 27 November 2026: the baseline wrote at the hour price and a later arm at
    the five-minute price, which made the later arm look 20 percent cheaper than it was). The five prices are
    fitted from the sessions' own reported costs, which they reproduce exactly, and each session is then priced
    again with all its cache writes at the hour price."""
    import numpy

    known = [row for row in rows if row.get("usage")]
    if len(known) < 5:
        return {"fitted": False, "sessions": len(known)}
    keys = ("input", "output", "cache_read", "write_5m", "write_1h")
    matrix = numpy.array([[row["usage"][key] for key in keys] for row in known], dtype=float)
    costs = numpy.array([row["usage"]["cost"] for row in known])
    prices, *_ = numpy.linalg.lstsq(matrix, costs, rcond=None)
    error = float(numpy.abs(matrix @ prices - costs).max())
    hour = prices[4] if any(row["usage"]["write_1h"] for row in known) else prices[3]
    for row in known:
        usage = row["usage"]
        row["cost_one_price"] = float(usage["input"] * prices[0] + usage["output"] * prices[1] + usage["cache_read"] * prices[2]
                                      + (usage["write_5m"] + usage["write_1h"]) * hour)
    return {"fitted": True, "sessions": len(known), "usd_per_million": {key: round(float(price) * 1e6, 4) for key, price in zip(keys, prices)},
            "largest_fit_error_usd": error, "cache_writes_priced_at": round(float(hour) * 1e6, 4)}


def sessions(series: pathlib.Path) -> dict[tuple[str, str], list[dict]]:
    found: dict[tuple[str, str], list[dict]] = {}
    for path in sorted(series.glob("*/session.json")):
        task_id, arm, _ = path.parent.name.rsplit(".", 2)
        row = json.loads(path.read_text(encoding="utf-8"))
        row["usage"] = final_usage(path.parent)
        found.setdefault((task_id, arm), []).append(row)
    return found


def medians(rows: list[dict]) -> dict:
    done = [row for row in rows if row.get("completed") and row.get("cost_usd") is not None]
    if not done:
        return {"runs": len(rows), "completed": 0}
    tokens = lambda key: statistics.median(row["tokens"].get(key) or 0 for row in done)  # noqa: E731
    return {"runs": len(rows), "completed": len(done),
            "cost_usd": round(statistics.median(row.get("cost_one_price", row["cost_usd"]) for row in done), 4),
            "reported_cost_usd": round(statistics.median(row["cost_usd"] for row in done), 4),
            "five_minute_share_of_cache_writes": round(
                sum(row["usage"]["write_5m"] for row in done if row.get("usage"))
                / max(1, sum(row["usage"]["write_5m"] + row["usage"]["write_1h"] for row in done if row.get("usage"))), 3),
            "round_trips": statistics.median(row["round_trips"] for row in done),
            "elapsed_s": round(statistics.median((row.get("elapsed_ms") or 0) / 1000 for row in done), 1),
            "cache_read": tokens("cache_read"), "cache_write": tokens("cache_write"), "output": tokens("output"),
            "gather_calls": statistics.median(row["gather_context"]["calls"] for row in done),
            "gather_bytes": statistics.median(row["gather_context"]["reply_bytes"] for row in done)}


def report(args) -> int:
    """Medians per task and arm, and each other arm against the plain arm (of ``--baseline`` when given)."""
    series = args.campaign / "agent-cost" / args.series
    found = sessions(series)
    if args.baseline:
        for key, rows in sessions(args.campaign / "agent-cost" / args.baseline).items():
            if key[1] == "plain":
                found.setdefault(key, rows)
    pricing = one_price([row for rows in found.values() for row in rows])
    print(json.dumps({"pricing": pricing}))
    weights = {row["id"]: (NARROW_WEIGHT if row["set"] == "narrow" else 1) for row in working_set(args.campaign)}
    arms = ["plain"] + sorted({key[1] for key in found} - {"plain"})
    table, saved = [], {arm: [] for arm in arms[1:]}
    for task_id in sorted({key[0] for key in found}):
        row = {"task": task_id, "weight": weights.get(task_id, 1)}
        text = f"{task_id:<22} w={row['weight']}"
        for arm in arms:
            if (task_id, arm) not in found:
                continue
            m = row[arm] = medians(found[(task_id, arm)])
            if not m.get("completed"):
                text += f" | {arm}: 0/{m['runs']} completed"
                continue
            text += (f" | {arm}: ${m['cost_usd']:.4f} trips={m['round_trips']} ({m['completed']}/{m['runs']})"
                     + (f" calls={m['gather_calls']} bytes={m['gather_bytes']}" if arm != "plain" else ""))
        plain = row.get("plain", {})
        for arm in arms[1:]:
            other = row.get(arm, {})
            if plain.get("cost_usd") and other.get("cost_usd") is not None:
                cost = round(100 * (1 - other["cost_usd"] / plain["cost_usd"]), 1)
                trips = round(100 * (1 - other["round_trips"] / plain["round_trips"]), 1)
                row[f"{arm}_cost_lower_pct"], row[f"{arm}_round_trips_lower_pct"] = cost, trips
                saved[arm].append((row["weight"], cost, trips, plain["cost_usd"], other["cost_usd"]))
                text += f" | {arm} cost lower {cost}% trips lower {trips}%"
        table.append(row)
        print(text)
    summary = {}
    for arm, rows in saved.items():
        if not rows:
            continue
        total = sum(weight for weight, *_ in rows)
        summary[arm] = {"tasks_compared": len(rows),
                        "weighted_cost_lower_pct": round(sum(weight * cost for weight, cost, *_ in rows) / total, 1),
                        "weighted_round_trips_lower_pct": round(sum(weight * trips for weight, _, trips, *_ in rows) / total, 1),
                        "weighted_plain_cost_usd": round(sum(weight * plain for weight, _, _, plain, _ in rows) / total, 4),
                        "weighted_arm_cost_usd": round(sum(weight * other for weight, _, _, _, other in rows) / total, 4),
                        "weighted_dollars_lower_pct": round(100 * (1 - sum(weight * other for weight, _, _, _, other in rows)
                                                                   / sum(weight * plain for weight, _, _, plain, _ in rows)), 1),
                        "five_minute_share_of_cache_writes": {
                            "plain": round(statistics.mean(row["plain"]["five_minute_share_of_cache_writes"] for row in table if arm in row and "plain" in row and row["plain"].get("completed")), 3),
                            arm: round(statistics.mean(row[arm]["five_minute_share_of_cache_writes"] for row in table if arm in row and row[arm].get("completed")), 3)}}
    if summary:
        print(json.dumps(summary))
    (series / "report.json").write_text(json.dumps({"schema": "airk.private.agent-cost-report/1", "series": args.series,
                                                     "baseline": args.baseline, "pricing": pricing, "tasks": table, "summary": summary},
                                                    indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
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
    parser.add_argument("--description-file", type=pathlib.Path, help="serve this tool description in place of the build's own")
    parser.add_argument("--tool-file", type=pathlib.Path,
                        help="a saved tool listing (name, description, inputSchema), so the oracle and live arms need no fixture")
    parser.add_argument("--tasks", nargs="*")
    parser.add_argument("--maps", type=pathlib.Path, help="folder of <snapshot>.map.json topical maps, for the live arm")
    parser.add_argument("--live-layout", default="full", help="live arm: full or focus")
    parser.add_argument("--oracle-layout", default="passages", help="oracle arm: passages or ranges")
    parser.add_argument("--oracle-trust", action="store_true", help="oracle arm: add the line saying what need not be read again")
    parser.add_argument("--baseline", help="report: take the plain arm from this series when this one has none")
    parser.add_argument("--max-round-trips", type=int, default=0,
                        help="stop each session after this many round trips (to collect first calls cheaply)")
    parser.add_argument("--judge", type=pathlib.Path, help="state file of a running arbiter_judge_service.py")
    parser.add_argument("--dose", type=int, default=0, help="judgments a call the model spends choosing passages")
    args = parser.parse_args(argv)
    if args.command == "report":
        return report(args)
    if not args.model or ("tool" in args.arms and args.handle is None) or (
            set(args.arms) & {"oracle", "live"} and args.handle is None and args.tool_file is None):
        parser.error("run needs --model, and --handle for the tool arm")
    if args.dose > 0 and args.judge is None:
        parser.error("--dose needs --judge")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
