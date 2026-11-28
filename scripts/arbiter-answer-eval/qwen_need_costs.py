# SPDX-License-Identifier: Apache-2.0
"""Summarize actual scorer work, frozen-control drift, and the two independent fixed-budget timing checks."""

import argparse
import json
from pathlib import Path
from statistics import median
import sys

import common


def summarize(path):
    header, *rows = common.read_jsonl(path)
    if header["type"] != "run" or sorted(r["task"] for r in rows) != sorted(header["tasks"]) or len(rows) != 20:
        raise ValueError("Expected twenty completed task rows from one paired replay")
    totals = {}
    for arm in header["arms"]:
        arms = [r["variants"][arm] for r in rows]
        totals[arm] = {"logical_pairs": sum(v["pairs"] for v in arms), "executed_pairs": sum(v["executed_pairs"] for v in arms),
                       "logical_clipped": sum(v["clipped"] for v in arms), "executed_clipped": sum(v["executed_clipped"] for v in arms),
                       "incremental_scoring_seconds": sum(v["model_seconds"] for v in arms),
                       "median_incremental_scoring_seconds_per_task": median(v["model_seconds"] for v in arms),
                       "arm_wall_seconds": sum(v["arm_wall_seconds"] for v in arms),
                       "delivered_bytes": sum(v["passage_bytes"] for v in arms)}
    comparisons = {}
    for arm in header["arms"]:
        if arm == "control":
            continue
        pairs = []
        for row in rows:
            base, test = row["variants"]["control"], row["variants"][arm]
            elapsed = (test["standalone_timing"]["model_seconds"] if "standalone_timing" in test else
                       test["model_seconds"] if test["executed_pairs"] == test["pairs"] else None)
            pairs.append({"task": row["task"], "logical_pair_delta": test["pairs"] - base["pairs"],
                          "passage_bytes_delta": test["passage_bytes"] - base["passage_bytes"],
                          "standalone_seconds_delta": elapsed - base["model_seconds"] if elapsed is not None else None,
                          "standalone_seconds_ratio": elapsed / base["model_seconds"] if elapsed is not None else None})
        measured = [p["standalone_seconds_ratio"] for p in pairs if p["standalone_seconds_ratio"] is not None]
        comparisons[arm] = {"reference": "control", "paired": pairs, "comparable_latency_tasks": len(measured),
                            "median_matched_scoring_time_ratio": median(measured) if measured else None}
    timing = {r["task"]: r["variants"]["fixed40"]["standalone_timing"] for r in rows
              if "standalone_timing" in r["variants"].get("fixed40", {})}
    if "fixed40" in header["arms"] and set(timing) != {"bookbuilder-A1", "munarium-A1"}:
        raise ValueError("The two independent fixed-allocation checks must both be recorded")
    historical = {}
    for r in rows:
        controls = r["variants"]["control"]
        historical[r["task"]] = {**controls["control_check"], "delta": controls["historical_delta"],
                                 "deterministic_delta": r["variants"]["deterministic"]["historical_delta"]}
    return {"input_sha256": common.sha256_file(path), "tasks": len(rows),
            "model": header["model"], "arms": totals, "timing_controls": timing,
            "historical": historical, "comparisons": comparisons,
            "timing_basis": "Incremental model timing includes tokenization and batches actually executed. A fully cached arm has zero new judgments. Fixed-budget uncached scoring was separately timed only on the two named tasks."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = summarize(args.receipt)
    if args.output:
        with args.output.open("x", encoding="utf-8") as fp:
            json.dump(result, fp, indent=2)
            fp.write("\n")
    else:
        json.dump(result, sys.stdout, indent=2)
        print()


if __name__ == "__main__":
    main()
