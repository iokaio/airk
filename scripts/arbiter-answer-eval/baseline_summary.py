# SPDX-License-Identifier: Apache-2.0
"""Hand-back summary of a graded pass: per-scenario table, headroom, judges, tokens, depth, cost.

It reads only what the pass wrote: ``results.csv``, ``summary.json``,
``pass-report.json``, ``settled.jsonl``, the judge verdicts and, if given, a
``depth.json``. It writes ``<pass>/baseline-summary.json`` and prints a
Markdown table. Analysis only: it runs no tool and no model.

``python -I -B baseline_summary.py --pass-dir <R>/passes/<series> [--depth <dir>/depth.json]``
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import grade  # noqa: E402


def judge_agreement(index: dict) -> dict:
    grades = facts = facts_agree = 0
    same_grade = 0
    for row in index["results"]:
        if row.get("reused_from"):
            continue
        verdicts = []
        for path in row.get("judges", [])[:2]:
            record = common.read_json(path) if Path(path).is_file() else None
            verdicts.append(record.get("verdict") if record and record.get("status") == "valid" else None)
        if len(verdicts) < 2 or None in verdicts:
            continue
        a = {n["id"]: n["verdict"] for n in verdicts[0].get("notes") or []}
        b = {n["id"]: n["verdict"] for n in verdicts[1].get("notes") or []}
        for note_id in a:
            if note_id in b:
                facts += 1
                facts_agree += a[note_id] == b[note_id]
        grades += 1
    return {"results_with_two_verdicts": grades, "fact_pairs": facts,
            "fact_agreement": round(facts_agree / facts, 4) if facts else None}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pass-dir", type=Path, required=True)
    parser.add_argument("--depth", type=Path)
    parser.add_argument("--recompute", metavar="SUFFIX",
                        help="regrade with the current grade.py (e.g. the tie rule) and write *-SUFFIX.json beside the "
                             "originals, which stay as they are")
    args = parser.parse_args(argv)
    d = args.pass_dir
    index = common.read_json(d / "pass-index.json")
    summary = common.read_json(d / "summary.json")
    reference_choice = None
    if args.recompute:
        adj = d / "adjudications.json"
        regraded = grade.build_rows(index, common.read_json(adj) if adj.is_file() else {})
        summary = grade.summarize(regraded, index["series"])
        common.write_json(d / f"summary-{args.recompute}.json", summary)
        reference_choice = []
        by = {}
        for r in regraded:
            by.setdefault(f"{r['repo']}-{r['slot']}", {})[r["configuration"]] = r
        for sid, rows in sorted(by.items()):
            chosen = grade.better_of([rows.get("search"), rows.get("parse")])
            ref = next((x for x in index["results"] if x["configuration"] == "reference-sonnet-best"
                        and f"{x['repo']}-{x['slot']}" == sid), None)
            used = ref.get("path") if ref else None
            if chosen is not None:
                reference_choice.append({"scenario": sid, "rule_choice": chosen["configuration"], "reference_ran": used,
                                         "differs": used is not None and used != chosen["configuration"],
                                         "search_covered": (rows.get("search") or {}).get("essential_covered"),
                                         "parse_covered": (rows.get("parse") or {}).get("essential_covered")})
    report = common.read_json(d / "pass-report.json")
    rows = list(csv.DictReader((d / "results.csv").open(encoding="utf-8")))
    grade_pairs = [(r["judge1_grade"], r["judge2_grade"]) for r in rows if r["judge1_grade"] and r["judge2_grade"]
                   and not r["reused_from"]]
    snaps = common.read_jsonl(d / "settled.jsonl")
    tokens = {}
    for snap in snaps:
        tokens.setdefault(snap["repo"], []).append(snap.get("publication_token"))
    figures = {f["key"]: f for f in report["results"]}
    table = []
    for r in rows:
        key = f"{r['repo']}-{r['slot']}/{r['configuration']}" + ("" if r["device"] in ("", "none") else f"-{r['device']}")
        f = figures.get(key, {})
        table.append({"scenario": f"{r['repo']}-{r['slot']}", "result": key.split("/", 1)[1],
                      "turns_to_finish": r["turns_to_finish"] or "-", "first_call_yield": r["first_call_yield"],
                      "essential": f"{r['essential_covered']}/{r['essential_total']}",
                      "j1": r["judge1_grade"], "j2": r["judge2_grade"], "adj": r["adjudicated_grade"],
                      "turns": f.get("turns"), "bytes": r["bytes"], "wall_s": round((f.get("wall_ms") or 0) / 1000),
                      "tool_s": round(int(r["elapsed_ms"] or 0) / 1000), "agent_usd": round(float(r["cost_usd"] or 0), 4),
                      "judge_usd": f.get("judge_cost_usd"), "reused_from": r["reused_from"] or ""})
    depth = None
    if args.depth and args.depth.is_file():
        data = common.read_json(args.depth)
        depth = {"limit_used": data["limit_used"], "token_unchanged": data["token_unchanged"],
                 "rows": [{"scenario": x["scenario_id"], "in_pool_rule2": x.get("essential_in_pool"),
                           "essential": x.get("essential_total"), "deepest_rank": x.get("deepest_rank"),
                           "ranks_rule2": [n["rule2"] for n in x.get("notes", [])],
                           "ranks_rule1": [n["rule1"] for n in x.get("notes", [])],
                           "ranks_same_path": [n["same_path"] for n in x.get("notes", [])]} for x in data["rows"]]}
    out = {"series": index["series"], "generated": common.now_iso(), "headroom": summary["headroom"],
           "configurations": {k: {"correct_finishes": v["correct_finishes"], "rule": v["rule"],
                                  "rubric_b_mean": v["rubric_b_mean"], "rubric_b_best_mean": v["rubric_b_best_mean"]}
                              for k, v in summary["configurations"].items()},
           "judge_grade_agreement": {"pairs": len(grade_pairs), "equal": sum(a == b for a, b in grade_pairs),
                                     "within_one": sum(abs(int(a) - int(b)) <= 1 for a, b in grade_pairs)},
           "judge_fact_agreement": judge_agreement(index),
           "pending_adjudication": summary["pending_adjudication"], "invalid": summary.get("invalid"),
           "tokens": {repo: {"first": t[0], "last": t[-1], "all_equal": len(set(t)) == 1, "snapshots": len(t)}
                      for repo, t in tokens.items()},
           "cost_usd": {"agents": report["totals"]["agent_cost_usd"], "judges": report["totals"]["judge_cost_usd"],
                        "total": round(report["totals"]["agent_cost_usd"] + report["totals"]["judge_cost_usd"], 4)},
           "per_result_name": report["per_result_name"], "table": table, "depth": depth,
           "reused": summary["reused"], "reference_choice_under_rule": reference_choice}
    common.write_json(d / ("baseline-summary.json" if not args.recompute else f"baseline-summary-{args.recompute}.json"), out)
    cols = ["scenario", "result", "turns_to_finish", "first_call_yield", "essential", "j1", "j2", "adj", "turns",
            "bytes", "wall_s", "tool_s", "agent_usd", "judge_usd", "reused_from"]
    print("| " + " | ".join(cols) + " |")
    print("|" + "---|" * len(cols))
    for row in table:
        print("| " + " | ".join(str(row[c]) for c in cols) + " |")
    print(json.dumps({k: out[k] for k in ("headroom", "judge_grade_agreement", "judge_fact_agreement", "tokens",
                                          "cost_usd", "pending_adjudication", "invalid")}, indent=1))
    if depth:
        print(json.dumps(depth, indent=1))
    if reference_choice is not None:
        print(json.dumps(reference_choice, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
