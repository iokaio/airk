# SPDX-License-Identifier: Apache-2.0
"""Hand-back summary of a graded pass: per-scenario table, headroom, judges, tokens, depth, cost.

It reads only what the pass wrote: ``pass-index.json``, ``summary.json``,
``pass-report.json``, ``settled.jsonl``, the judge verdicts and, if given, a
``depth.json``. It writes ``<pass>/baseline-summary.json`` and prints a
Markdown table. With --l7-metrics it prints only aggregate sample-one L7
metrics without writing to the pass. It runs no tool and no model.

``python -I -B baseline_summary.py --pass-dir <R>/passes/<series> [--depth <dir>/depth.json]``
"""

from __future__ import annotations

import argparse
from collections import Counter
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import grade  # noqa: E402
import loop_reply  # noqa: E402


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


L7_CONFIGURATIONS = {("search", "none"), ("parse", "none"), ("focus", "gpu"),
                     ("compact", "gpu"), ("arbiter", "gpu")}


def l7_cells(index: dict, adjudications: dict) -> list[dict]:
    """Aggregate public metrics from sample-one sessions; only fixed L7 labels leave this reader."""
    entries = [entry for entry in index["results"] if entry.get("sample", 1) == 1]
    by_key = {entry["key"]: entry for entry in entries}
    rows = grade.build_rows({**index, "results": entries}, adjudications)
    buckets = {}
    observed = set()
    for row in rows:
        repo, configuration, device = (row[key] for key in ("repo", "configuration", "device"))
        if repo not in common.REPOS or (configuration, device) not in L7_CONFIGURATIONS:
            continue
        cell = repo, configuration, device
        identity = cell, row["slot"]
        if identity in observed:
            raise ValueError("Duplicate L7 sample-one result")
        observed.add(identity)
        bucket = buckets.setdefault(cell, Counter())
        bucket["sessions"] += 1

        entry = by_key[row["key"]]
        source = by_key.get(entry.get("reused_from"), entry)
        session_dir = Path(source["session_dir"])
        session = common.read_json(session_dir / "session.json")
        if isinstance(session.get("elapsed_ms"), int):
            bucket["wall_sessions"] += 1
            bucket["wall_ms"] += session["elapsed_ms"]
        if row["essential_total"] is not None:
            coverage = common.read_json(source["coverage"])
            bucket["coverage_sessions"] += 1
            bucket["essential"] += row["essential_covered"]
            bucket["essential_total"] += row["essential_total"]
            bucket["reply_bytes"] += row["bytes"]
            bucket["tool_ms"] += row["elapsed_ms"]
            supporting = set(coverage["helpful"])
            bucket["supporting"] += len(supporting.intersection(coverage["covered_any"]))
            bucket["supporting_total"] += len(supporting)
            later = coverage.get("later_reads")
            if later is not None:
                bucket["reread_sessions"] += 1
                bucket["rereads_truncated"] += later["to_truncated_row"]
                bucket["rereads_whole"] += later["to_whole_row"]
        turn_path = session_dir / "wrapper" / "turns.jsonl"
        if turn_path.is_file():
            bucket["wrapper_sessions"] += 1
            for turn in common.read_jsonl(turn_path):
                l7_count_turn(bucket, turn)

    return [{"repo": repo, "configuration": configuration, "device": device, "counts": counts}
            for (repo, configuration, device), counts in sorted(buckets.items())]


def l7_count_turn(bucket: Counter, turn: dict) -> None:
    if turn.get("rejected"):
        bucket["wrapper_refused"] += 1
        return
    if not turn.get("turn"):
        return
    bucket["tool_calls"] += 1
    bucket["tool_failed"] += turn.get("ok") is False
    for attempt in turn.get("typed_retries") or []:
        name = "ticket_attaches" if attempt.get("code") == "ticket" else "typed_retries"
        bucket[name] += 1
    if turn.get("tool") != "airk_answer" or not turn.get("ok") or not turn.get("result_path"):
        return
    result = Path(turn["result_path"])
    body = common.raw_body(result.read_bytes()) if result.is_file() else None
    if not loop_reply.is_v3(body) or loop_reply.is_ticket(body):
        return
    bucket["answer_replies"] += 1
    partial = body.get("partial")
    bucket["deadline_partials"] += isinstance(partial, dict) and partial.get("reason") == "deadline"
    log = body.get("decision_log")
    if isinstance(log, dict) and isinstance(log.get("counts"), list):
        bucket["decision_replies"] += 1
        for row in loop_reply.log_counts(body):
            bucket["decision_" + loop_reply.source_class(row["source"])] += row["count"]


def l7_markdown(rows: list[dict]) -> str:
    headers = ("Repository", "Configuration", "Sessions", "Essential", "Supporting", "Reply bytes",
               "Rereads truncated / whole", "Failed / calls", "Wrapper refusals", "Typed retries / tickets",
               "Deadline partials / Answer replies", "Decision sources", "Mean wall s", "Mean tool s",
               "Coverage / reread / wrapper sessions")
    lines = ["| " + " | ".join(headers) + " |", "|" + "---|" * len(headers)]
    for row in rows:
        count = row["counts"]
        coverage, rereads, wrappers = (count[k] for k in ("coverage_sessions", "reread_sessions", "wrapper_sessions"))
        sources = [str(count["decision_" + source]) for source in loop_reply.SOURCE_CLASS_ORDER]
        fields = [
            row["repo"], row["configuration"] + ("-gpu" if row["device"] == "gpu" else ""),
            count["sessions"],
            f"{count['essential']}/{count['essential_total']}" if coverage else None,
            f"{count['supporting']}/{count['supporting_total']}" if coverage else None,
            count["reply_bytes"] if coverage else None,
            f"{count['rereads_truncated']}/{count['rereads_whole']}" if rereads else None,
            f"{count['tool_failed']}/{count['tool_calls']}" if wrappers else None,
            count["wrapper_refused"] if wrappers else None,
            f"{count['typed_retries']}/{count['ticket_attaches']}" if wrappers else None,
            f"{count['deadline_partials']}/{count['answer_replies']}" if count["answer_replies"] else None,
            "/".join(sources) + f" ({count['decision_replies']} replies)" if count["decision_replies"] else None,
            f"{count['wall_ms'] / count['wall_sessions'] / 1000:.2f}" if count["wall_sessions"] else None,
            f"{count['tool_ms'] / coverage / 1000:.2f}" if coverage else None,
            f"{coverage}/{rereads}/{wrappers}",
        ]
        lines.append("| " + " | ".join("-" if field is None else str(field) for field in fields) + " |")
    lines.append("")
    lines.append("Sample one only. Totals include reused sessions as outcomes for their configuration. "
                 "Decision sources count model / fell back / not dispatched / deterministic / other from the "
                 "native decision log of each observed non-ticket Answer reply, including continuations. "
                 "Peak memory is not measured here.")
    return "\n".join(lines)


def result_table(rows: list[dict], report: dict) -> list[dict]:
    figures = {f["key"]: f for f in report["results"]}
    table = []

    def csv_cell(value):
        return "" if value is None else str(value)

    for r in rows:
        key = r["key"]
        f = figures.get(key, {})
        table.append({"scenario": f"{r['repo']}-{r['slot']}", "result": key.split("/", 1)[1],
                      "turns_to_finish": csv_cell(r["turns_to_finish"]) or "-",
                      "first_call_yield": csv_cell(r["first_call_yield"]),
                      "essential": f"{csv_cell(r['essential_covered'])}/{csv_cell(r['essential_total'])}",
                      "j1": csv_cell(r["judge1_grade"]), "j2": csv_cell(r["judge2_grade"]),
                      "adj": csv_cell(r["adjudicated_grade"]),
                      "turns": f.get("turns"), "bytes": csv_cell(r["bytes"]), "wall_s": round((f.get("wall_ms") or 0) / 1000),
                      "tool_s": round((r["elapsed_ms"] or 0) / 1000), "agent_usd": round(r["cost_usd"] or 0, 4),
                      "judge_usd": f.get("judge_cost_usd"), "reused_from": r["reused_from"] or ""})
    return table


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pass-dir", type=Path, required=True)
    parser.add_argument("--depth", type=Path)
    parser.add_argument("--l7-metrics", action="store_true", help="print only the read-only sample-one L7 table")
    parser.add_argument("--recompute", metavar="SUFFIX",
                        help="regrade with the current grade.py (e.g. the tie rule) and write *-SUFFIX.json beside the "
                             "originals, which stay as they are")
    args = parser.parse_args(argv)
    d = args.pass_dir
    index = common.read_json(d / "pass-index.json")
    if args.l7_metrics:
        if args.depth or args.recompute:
            parser.error("--l7-metrics cannot be combined with --depth or --recompute")
        adj = d / "adjudications.json"
        print(l7_markdown(l7_cells(index, common.read_json(adj) if adj.is_file() else {})))
        return 0
    summary = common.read_json(d / "summary.json")
    adj = d / "adjudications.json"
    rows = grade.build_rows(index, common.read_json(adj) if adj.is_file() else {})
    reference_choice = None
    if args.recompute:
        frozen_path = d / grade.FIRST_GRADE_SECOND_JOBS
        frozen = common.read_json(frozen_path)["second_sessions"] if frozen_path.is_file() else None
        summary = grade.summarize(rows, index["series"], frozen)
        common.write_json(d / f"summary-{args.recompute}.json", summary)
        reference_choice = []
        by = {}
        for r in rows:
            if r["sample"] != 1:
                continue
            by.setdefault(f"{r['repo']}-{r['slot']}", {})[r["configuration"]] = r
        for sid, scenario_rows in sorted(by.items()):
            chosen = grade.better_of([scenario_rows.get("search"), scenario_rows.get("parse")])
            ref = next((x for x in index["results"] if x["configuration"] == "reference-sonnet-best"
                        and f"{x['repo']}-{x['slot']}" == sid), None)
            used = ref.get("path") if ref else None
            if chosen is not None:
                reference_choice.append({"scenario": sid, "rule_choice": chosen["configuration"], "reference_ran": used,
                                         "differs": used is not None and used != chosen["configuration"],
                                         "search_covered": (scenario_rows.get("search") or {}).get("essential_covered"),
                                         "parse_covered": (scenario_rows.get("parse") or {}).get("essential_covered")})
    report = common.read_json(d / "pass-report.json")
    grade_pairs = [(r["judge1_grade"], r["judge2_grade"]) for r in rows
                   if r["judge1_grade"] is not None and r["judge2_grade"] is not None and not r["reused_from"]]
    snaps = common.read_jsonl(d / "settled.jsonl")
    tokens = {}
    for snap in snaps:
        tokens.setdefault(snap["repo"], []).append(snap.get("publication_token"))
    table = result_table(rows, report)
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
                                     "within_one": sum(abs(a - b) <= 1 for a, b in grade_pairs)},
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
