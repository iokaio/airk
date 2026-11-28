# SPDX-License-Identifier: Apache-2.0
"""Join the frozen four-pair design, private actor invoices and sealed blind first/final grades."""
import argparse
import json
from pathlib import Path
from statistics import mean

import agent_cost_batch
import common


def one_price(usage):
    base = 2 * usage["input"] + 10 * usage["output"] + 0.1 * usage["cache_read"]
    raw = (base + 2.5 * usage["write_5m"] + 4 * usage["write_1h"]) / 1e6
    common.require(abs(raw - usage["cost"]) < 1e-7, "Invoice no longer matches reference token prices")
    return (base + 4 * (usage["write_5m"] + usage["write_1h"])) / 1e6


def row(series, task, arm, freeze):
    run = series / "runs" / f"{task['id']}.{arm}"
    receipt = common.read_json(run / "session.json")
    common.require(receipt["completed"] and receipt["exit_code"] == 0 and not receipt["timed_out"] and
                   receipt["model"] == "claude-sonnet-5-5" and
                   receipt["task"]["sha256"].lower() == freeze[f"solver/{task['id']}.json"].lower(),
                   "Session identity or completion differs from the frozen design")
    if arm == "candidate":
        common.require(receipt["product_surface_valid"] and receipt["companion_admission"]["valid"] and
                       (task["kind"] != "research" or receipt["research_frozen_index"]["valid"]),
                       "Incorrect candidate runtime or source binding")
    usage = agent_cost_batch.final_usage(run)
    common.require(abs(usage["cost"] - receipt["cost_usd"]) < 1e-8, "Session and invoice differ")
    proof = common.read_json(series / "blind-provenance" / (run.name + ".json"))
    grades = {}
    for mapping in proof:
        packet = mapping["packet"]
        grade_path = series / "independent-grades" / packet / "grade.json"
        grade = common.read_json(grade_path)
        common.require(grade["task"] == task["id"] and grade["packet"] == packet and
                       grade["rubric_sha256"].lower() == freeze[f"grading/{task['id']}.json"].lower(),
                       "Grade task or rubric differs")
        inputs = grade.get("inputs") or grade["input_files"]
        for name, digest in inputs.items():
            common.require(common.sha256_file(series / "blind-staging" / packet / name).lower() == digest.lower(),
                           "Graded artifact changed")
        if mapping["phase"] == "final":
            common.require(common.sha256_file(run / "changes.patch").lower() == mapping["patch_sha256"].lower(),
                           "Final patch does not match the blind mapping")
        grades[mapping["phase"]] = {"passed": grade["fully_correct"], "grade_sha256": common.sha256_file(grade_path)}
    common.require(set(grades) == {"first", "final"}, "Missing first or final grade")
    events = common.read_jsonl(run / "stream.jsonl")
    turns, sequence, tool_ids = {}, [], set()
    for event in events:
        if event.get("type") != "assistant":
            continue
        message = event["message"]
        turn = turns.setdefault(message["id"], len(turns) + 1)
        for block in message["content"]:
            if block.get("type") == "tool_use" and block["id"] not in tool_ids:
                tool_ids.add(block["id"])
                sequence.append({"round": turn, "tool": block["name"]})
    calls_file = run / "gather/calls.jsonl"
    calls = common.read_jsonl(calls_file) if calls_file.exists() else []
    gather_turns = [s["round"] for s in sequence if s["tool"] == "mcp__airk__gather_context"]
    common.require(len(turns) == receipt["round_trips"] and len(calls) == len(gather_turns) == receipt["gather_context"]["calls"],
                   "Request or Gather count differs between trace and kernel bridge receipts")
    gather = [{"round": turn, **{k: c[k] for k in ("wire_bytes", "reply_bytes", "judgments", "seconds", "is_error")}}
              for turn, c in zip(gather_turns, calls)]
    return {"task": task["id"], "arm": arm, "kind": task["kind"], "stratum": task["stratum"],
            "invoice_usd": usage["cost"], "common_price_usd": one_price(usage), "tokens": usage,
            "round_trips": len(turns), "tool_sequence": sequence, "gather": gather,
            "tool_result_bytes": receipt["tool_result_bytes"], "grades": grades,
            "session_sha256": common.sha256_file(run / "session.json"),
            "started_utc": (next((e.get("timestamp") for e in events if e.get("timestamp")), None))}


def aggregate(rows, weighted):
    values = {}
    for arm in ("plain", "candidate"):
        selected = [r for r in rows if r["arm"] == arm]
        pairs = [(4 if weighted and r["stratum"] == "narrow" else 1, r) for r in selected]
        values[arm] = {k: sum(w * r[k] for w, r in pairs) for k in ("common_price_usd", "round_trips", "invoice_usd")}
        values[arm]["weight"] = sum(w for w, _ in pairs)
        for phase in ("first", "final"):
            values[arm][phase + "_passed"] = sum(w * int(r["grades"][phase]["passed"]) for w, r in pairs)
    for metric in ("common_price_usd", "round_trips"):
        values[metric + "_percent_change"] = 100 * (values["candidate"][metric] / values["plain"][metric] - 1)
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("design", type=Path)
    parser.add_argument("series", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    freeze_file = args.design / "freeze.json"
    common.require(common.sha256_file(freeze_file) == "d3a7430c3167371830bbd3304f9430e2a7328cfcff4c53b4e5e30a00232c5a30",
                   "Unexpected task freeze")
    freeze = common.read_json(freeze_file)
    for name, digest in freeze.items():
        common.require(common.sha256_file(args.design / name).lower() == digest.lower(), "Frozen task input changed")
    tasks = common.read_json(args.design / "design.json")["tasks"]
    rows = [row(args.series, task, arm, freeze) for task in tasks for arm in ("plain", "candidate")]
    common.require(len(rows) == 8, "Expected four complete pairs")
    deltas = {}
    for task in tasks:
        pair = [r for r in rows if r["task"] == task["id"]]
        deltas[task["id"]] = {"stratum": task["stratum"],
                              "common_price_percent_change": 100 * (pair[1]["common_price_usd"] / pair[0]["common_price_usd"] - 1)}
    strata = {s: mean(r["common_price_percent_change"] for r in deltas.values() if r["stratum"] == s) for s in ("broad", "narrow")}
    report = {"schema": "airk.eval.prospective-four-pairs/1", "usd_per_million": {
        "input": 2, "output": 10, "cache_read": 0.1, "all_cache_writes_normalized": 4, "raw_5m_write": 2.5},
        "paid_rows": rows, "unweighted": aggregate(rows, False), "fourfold_narrow": aggregate(rows, True),
        "per_task": deltas, "mean_task_pct_change_by_stratum": strata,
        "fourfold_mean_of_task_pct_changes": (strata["broad"] + 4 * strata["narrow"]) / 5}
    with args.output.open("x", encoding="utf-8") as out:
        json.dump(report, out, indent=2)
    print(json.dumps({k: report[k] for k in ("unweighted", "fourfold_narrow", "fourfold_mean_of_task_pct_changes")}))


if __name__ == "__main__":
    main()