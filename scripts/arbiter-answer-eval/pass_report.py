# SPDX-License-Identifier: Apache-2.0
"""Per-session and per-path figures of a graded pass, for reports and cost projections.

Reads ``<R>/passes/<series>/pass-index.json`` (and the session, coverage and
verdict records it points at) and ``results.csv``. Writes
``pass-report.json`` and prints two tables:

* every result: turns, refused calls, bytes, tool time, session wall time,
  coverage turn, judge and adjudicated grades, agent and judge cost, failed
  tool calls;
* every result name (path and device): sessions run, mean and maximum wall
  time, mean agent cost, mean judge cost per result, mean turns, mean tool
  time per call of the primary tool, failure counts.

``python -I -B pass_report.py --pass-dir <R>/passes/<series>``
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path
from statistics import mean

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402


def session_figures(row: dict) -> dict:
    directory = Path(row["session_dir"])
    record = common.read_json(directory / "session.json") if (directory / "session.json").is_file() else {}
    turns = common.read_jsonl(directory / "wrapper" / "turns.jsonl")
    counted = [t for t in turns if t.get("turn")]
    path = record.get("path") or row["path"]
    primary = common.PATH_TOOLS[path][0] if path in common.PATH_TOOLS else None
    first = counted[0] if counted else {}
    judge_cost = 0.0
    if not row.get("reused_from"):
        for path in row.get("judges", []):
            if Path(path).is_file():
                judge_cost += common.read_json(path).get("cost_usd") or 0.0
    return {
        "key": row["key"], "result": row["key"].split("/", 1)[1], "status": record.get("status"),
        "reused_from": row.get("reused_from"), "turns": len(counted), "refused": len(turns) - len(counted),
        "failed_calls": sum(1 for t in counted if not t.get("ok")),
        "failed_primary_calls": sum(1 for t in counted if not t.get("ok") and t.get("tool") == primary),
        "primary_calls": sum(1 for t in counted if t.get("tool") == primary),
        "primary_ms": [t.get("elapsed_ms") for t in counted if t.get("tool") == primary],
        "first_call_ok": first.get("ok"), "first_call_ms": first.get("elapsed_ms"), "first_call_bytes": first.get("bytes"),
        "bytes": sum(int(t.get("bytes") or 0) for t in counted),
        "tool_ms": sum(int(t.get("elapsed_ms") or 0) for t in counted),
        "wall_ms": record.get("elapsed_ms"), "agent_cost_usd": record.get("cost_usd") or 0.0,
        "judge_cost_usd": round(judge_cost, 4),
        "unaudited_bare_refs": ((record.get("audit") or {}).get("citations") or {}).get("unaudited_bare_refs"),
        "unbacked": len(((record.get("audit") or {}).get("citations") or {}).get("unbacked") or []),
    }


def aggregate(figures: list[dict]) -> dict:
    by: dict[str, list[dict]] = {}
    for f in figures:
        if not f.get("reused_from") and f.get("status"):
            by.setdefault(f["result"], []).append(f)
    table = {}
    for name, rows in sorted(by.items()):
        primary = [ms for r in rows for ms in r["primary_ms"] if ms is not None]
        table[name] = {"sessions": len(rows), "wall_ms_mean": round(mean(r["wall_ms"] or 0 for r in rows)),
                       "wall_ms_max": max(r["wall_ms"] or 0 for r in rows),
                       "agent_cost_usd_mean": round(mean(r["agent_cost_usd"] for r in rows), 4),
                       "judge_cost_usd_mean": round(mean(r["judge_cost_usd"] for r in rows), 4),
                       "turns_mean": round(mean(r["turns"] for r in rows), 2),
                       "tool_ms_mean": round(mean(r["tool_ms"] for r in rows)),
                       "primary_call_ms_mean": round(mean(primary)) if primary else None,
                       "primary_call_ms_max": max(primary) if primary else None,
                       "failed_primary_calls": sum(r["failed_primary_calls"] for r in rows),
                       "primary_calls": sum(r["primary_calls"] for r in rows),
                       "bytes_mean": round(mean(r["bytes"] for r in rows))}
    return table


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pass-dir", type=Path, required=True)
    args = parser.parse_args(argv)
    index = common.read_json(args.pass_dir / "pass-index.json")
    grades = {}
    csv_path = args.pass_dir / "results.csv"
    if csv_path.is_file():
        for row in csv.DictReader(csv_path.open(encoding="utf-8")):
            grades[f"{row['repo']}-{row['slot']}/{row['configuration']}" + (
                "" if row["device"] in ("", "none") else f"-{row['device']}")] = row
    figures = []
    for row in index["results"]:
        if row.get("sample", 1) != 1 and not Path(row["session_dir"]).is_dir():
            continue
        f = session_figures(row)
        g = grades.get(row["key"], {})
        coverage_path = Path(row["coverage"])
        cov = common.read_json(coverage_path) if coverage_path.is_file() else {}
        f.update(coverage_turn=cov.get("all_essential_turn"), essential=f"{cov.get('essential_covered')}/"
                 f"{cov.get('essential_total')}", first_call_yield=cov.get("first_call_yield"),
                 j1=g.get("judge1_grade"), j2=g.get("judge2_grade"), adjudicated=g.get("adjudicated_grade"),
                 correct_finish=g.get("correct_finish"), turns_to_finish=g.get("turns_to_finish"))
        figures.append(f)
    report = {"series": index["series"], "generated": common.now_iso(), "results": figures,
              "per_result_name": aggregate(figures),
              "totals": {"agent_cost_usd": round(sum(f["agent_cost_usd"] for f in figures if not f["reused_from"]), 4),
                         "judge_cost_usd": round(sum(f["judge_cost_usd"] for f in figures), 4),
                         "sessions": sum(1 for f in figures if not f["reused_from"] and f["status"])}}
    common.write_json(args.pass_dir / "pass-report.json", report)
    for f in figures:
        print(json.dumps({k: f[k] for k in ("key", "status", "turns", "refused", "failed_calls", "bytes", "tool_ms",
                                            "wall_ms", "coverage_turn", "essential", "j1", "j2", "adjudicated",
                                            "agent_cost_usd", "judge_cost_usd", "reused_from")}))
    print(json.dumps(report["per_result_name"], indent=1))
    print(json.dumps(report["totals"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
