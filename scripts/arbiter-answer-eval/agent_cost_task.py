# SPDX-License-Identifier: Apache-2.0
"""Compare the conditions run on one task, every repeat shown.

Evaluation tooling for the one-task-at-a-time experiments of
``docs/arbiter/topical-index-research.md``. A condition is one series and
arm. For one task it prints each condition's repeats at one price for every
cache write (``agent_cost_batch.one_price``), the median, the spread, the
round trips, the bytes the agent read through its own tools, how often it
called Gather Context, and the median against the first condition named,
which is the unaided baseline. It judges nothing.

    python -B agent_cost_task.py --campaign <dir> --task munarium-T1 --conditions baseline-sonnet-r1:plain oracle-sonnet-r1:oracle ...
"""

from __future__ import annotations

import argparse
import json
import pathlib
import statistics
import sys

import agent_cost_batch


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--campaign", required=True, type=pathlib.Path)
    parser.add_argument("--task", required=True)
    parser.add_argument("--conditions", required=True, nargs="+", help="<series>:<arm>, the baseline first")
    args = parser.parse_args(argv)
    groups = []
    for condition in args.conditions:
        series, _, arm = condition.partition(":")
        rows = agent_cost_batch.sessions(args.campaign / "agent-cost" / series).get((args.task, arm), [])
        groups.append((condition, [row for row in rows if row.get("completed")]))
    pricing = agent_cost_batch.one_price([row for _, rows in groups for row in rows])
    print(json.dumps({"task": args.task, "pricing": pricing}))
    base = None
    table = []
    for condition, rows in groups:
        if not rows:
            print(f"{condition:<40} no completed sessions")
            continue
        costs = sorted(row.get("cost_one_price", row["cost_usd"]) for row in rows)
        median = statistics.median(costs)
        base = median if base is None else base
        own = statistics.median(row.get("own_read_bytes") or 0 for row in rows)
        trips = statistics.median(row["round_trips"] for row in rows)
        called = sum(row["gather_context"]["calls"] > 0 for row in rows)
        reply = statistics.median(row["gather_context"]["reply_bytes"] for row in rows)
        entry = {"condition": condition, "runs": len(rows), "median_usd": round(median, 4), "costs_usd": [round(cost, 3) for cost in costs],
                 "lower_than_first_pct": round(100 * (1 - median / base), 1), "round_trips": trips, "own_read_bytes": own,
                 "called": called, "reply_bytes": reply}
        table.append(entry)
        print(f"{condition:<40} n={len(rows)} median ${median:.3f} ({entry['lower_than_first_pct']:+.1f}% lower) "
              f"[{' '.join(f'{cost:.2f}' for cost in costs)}] trips {trips} own-read {own / 1000:.0f}KB called {called}/{len(rows)} reply {reply}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
